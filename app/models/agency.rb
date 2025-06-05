class Agency < ApplicationRecord
  self.primary_key = :id

  has_many :routes, dependent: :destroy
end
