class Service < ApplicationRecord
  self.primary_key = :id

  has_many :calendar_dates, dependent: :destroy
end
